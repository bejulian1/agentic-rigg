#!/usr/bin/env python3
"""Kort, styrt prøve av «Av» på kortlyset. Kjøres av brukeren selv etter
installasjonen, med øynene på kortet. Ingen root, ingen nye registre.

Hver kandidat er en vanlig, journalført RGB-prøve gjennom den installerte
RGB-hjelperen (samme socket og samme faste skrivesekvens som appen): den
skrives, holdes i HOLD sekunder og rulles så tilbake til lyset kortet hadde.
Uten svar ruller hjelperen selv tilbake etter fristen (30 s), også om
skriptet avbrytes. Tilbakelesingen viser bare registrene, ikke hva kortet
lyser; svaret ditt er selve målingen.

Kandidatene (bare registrene 60 og 6C–6F, som appens faste farge):
  1  svart-100     60=01, RGB 000000, lysstyrke 100 (dagens «Av»)
  2  svart-0       60=01, RGB 000000, lysstyrke 0
  3  farge-0       60=01, utgangsfargen, lysstyrke 0
  4  nesten-svart  60=01, RGB 010101, lysstyrke 1
  5  hold          som 2, men skrevet på nytt hvert 2. sekund (appens standard)

Bruk (lukk appen først, eller trykk «Avslutt lysstyring»):
  python3 rgb-av-prove.py --liste
  python3 rgb-av-prove.py --kandidat 2      # én kandidat, ca. 20 s
  python3 rgb-av-prove.py --bekreft          # etter svaret: lyset er som før
  python3 rgb-av-prove.py --alle             # alle fem, spør i terminalen

Etter hver kandidat er kortet tilbake på utgangslyset, men hjelperen venter
på at et menneske bekrefter det. `--bekreft` lagrer den bekreftelsen; først
da kan neste kandidat prøves.
"""
import argparse
from contextlib import closing
import json
import os
import socket
import sys
import time

SOCKET = '/run/agentic-rigg-rgb/helper.sock'
HOLD = 15
DEADLINE_S = 30
HEARTBEAT_S = 5
CANDIDATES = {
    1: ('svart-100', 'svart med lysstyrke 100 (dagens «Av»)', lambda base: {'mode': 'off'}),
    2: ('svart-0', 'svart med lysstyrke 0', lambda base: {'mode': 'static', 'color': [0, 0, 0], 'brightness': 0}),
    3: ('farge-0', 'utgangsfargen med lysstyrke 0', lambda base: {'mode': 'static', 'color': base, 'brightness': 0}),
    4: ('nesten-svart', 'nesten svart (010101) med lysstyrke 1', lambda base: {'mode': 'static', 'color': [1, 1, 1], 'brightness': 1}),
    5: ('hold', 'svart med lysstyrke 0, skrevet på nytt hvert 2. sekund (appens standard for «Av»)',
        lambda base: {'mode': 'static', 'color': [0, 0, 0], 'brightness': 0}),
}
# Kandidaten som prøver appens «hold» (keepalive) i stedet for én skriving.
HOLD_CANDIDATE = 5


class Feil(Exception):
    pass


class Hjelper:
    """Én forbindelse til RGB-hjelperen. Lukkes forbindelsen under en prøve,
    ruller hjelperen selv tilbake."""

    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(20)
        try:
            self.sock.connect(path)
        except OSError as error:
            self.sock.close()
            raise Feil(f'Fikk ikke kontakt med RGB-hjelperen på {path} ({error}). '
                       'Er kortlyset aktivert i appen?') from None
        self.buffer = b''
        self.next_id = 1

    def call(self, op, args=None):
        rid = self.next_id
        self.next_id += 1
        self.sock.sendall((json.dumps({'id': rid, 'op': op, 'args': args or {}}) + '\n').encode())
        while b'\n' not in self.buffer:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise Feil('RGB-hjelperen lukket forbindelsen.')
            self.buffer += chunk
        line, self.buffer = self.buffer.split(b'\n', 1)
        reply = json.loads(line)
        if reply.get('id') != rid:
            raise Feil('Uventet svar fra RGB-hjelperen.')
        if not reply.get('ok'):
            error = reply.get('error') or {}
            raise Feil(f"{error.get('code')}: {error.get('message')}")
        return reply['result']


def card(status):
    identity = status.get('identity') or {}
    return {'gpu': identity.get('pci'), 'uuid': identity.get('uuid')}


def describe(request):
    if request.get('mode') == 'off':
        return 'av (svart, 100 %)'
    r, g, b = request['color']
    return f"#{r:02X}{g:02X}{b:02X} · {request['brightness']} %"


def ready(status):
    state = status.get('state') or {}
    if status.get('authorized') is not True:
        raise Feil('RGB-tilgangen er ikke aktiv. Aktiver kortlyset i appen først.')
    if state.get('recoveryNeedsConfirmation'):
        raise Feil('En gjenoppretting venter på bekreftelse. Se på kortet, og kjør --bekreft '
                   'når lyset er som før.')
    if (state.get('pending') or state.get('owner') or status.get('effect')
            or status.get('holding') or status.get('keepalive')):
        raise Feil('Kortlyset eies av appen eller en annen prøve. Trykk «Avslutt lysstyring» '
                   'i appen eller lukk appen, og prøv igjen.')
    if state.get('dirty') or state.get('error') or state.get('readError'):
        raise Feil('RGB-økten er ikke ren. Avklar den på siden RGB-lys først.')
    confirmed = state.get('confirmed') or {}
    if (confirmed.get('mode') != 'static' or confirmed.get('color') == [0, 0, 0]
            or not confirmed.get('brightness')):
        raise Feil('Kortet står ikke på en synlig fast farge nå. Velg en fast farge med lysstyrke '
                   'over 0 i appen og trykk «Behold» først, så forskjellen kan ses.')
    return confirmed


def run_candidate(path, number, hold, out=print):
    helper = Hjelper(path)
    with closing(helper.sock):
        return _candidate(helper, number, hold, out)


def _candidate(helper, number, hold, out):
    name, text, build = CANDIDATES[number]
    status = helper.call('rgb_status')
    base = ready(status)
    request = build(base['color'])
    ids = card(status)
    out(f'Kandidat {number} ({name}): {text}.')
    out(f'Utgangslyset er {describe(base)}. Se på kortet nå.')
    result = helper.call('rgb_apply', {**ids, 'request': request, 'confirmTimeoutS': DEADLINE_S})
    txn = (result.get('outcome') or {}).get('txn')
    if not txn:
        raise Feil('Hjelperen startet ingen prøve (valget er likt utgangslyset?).')
    if number == HOLD_CANDIDATE:
        if result.get('status', {}).get('hold') is not True:
            helper.call('rgb_revert', {**ids, 'txn': txn})
            raise Feil('RGB-hjelperen støtter ikke «hold». Oppdater appen og kortlyset først.')
        # Samme prøve; hjelperen skriver den på nytt til den rulles tilbake.
        helper.call('rgb_hold', ids)
    started = time.monotonic()
    out(f'Skrevet: {describe(request)}. Holder i {hold} s …')
    while (left := hold - (time.monotonic() - started)) > 0:
        time.sleep(min(HEARTBEAT_S, left))
        if time.monotonic() - started < hold:
            helper.call('rgb_heartbeat', ids)
    helper.call('rgb_revert', {**ids, 'txn': txn})
    out('Rullet tilbake til utgangslyset.')
    return name, describe(base)


def confirm(path, out=print):
    helper = Hjelper(path)
    with closing(helper.sock):
        status = helper.call('rgb_status')
        state = status.get('state') or {}
        if not state.get('recoveryNeedsConfirmation') or not status.get('recoveryId'):
            out('Ingen gjenoppretting venter på bekreftelse.')
            return False
        helper.call('rgb_ack_recovery', {**card(status), 'confirmedStatic': True,
                                         'expected': state['confirmed'], 'recoveryId': status['recoveryId']})
        out(f"Bekreftet: kortet viser {describe(state['confirmed'])} igjen. Økten er ren.")
        return True


def question(name, base):
    return (f'SPØRSMÅL TIL BRUKEREN ({name}): Ble kortlyset slukket, og holdt det seg slukket '
            f'hele tiden til det kom tilbake nå? Svar «holdt», «kom tilbake etter ca. N sekunder» '
            f'eller «ble ikke slukket», og om kortet nå viser utgangslyset ({base}) igjen.')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument('--liste', action='store_true')
    action.add_argument('--kandidat', type=int, choices=sorted(CANDIDATES))
    action.add_argument('--bekreft', action='store_true')
    action.add_argument('--alle', action='store_true')
    parser.add_argument('--socket', default=SOCKET, help=argparse.SUPPRESS)
    parser.add_argument('--holdetid', type=float, default=HOLD, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    if not 0.2 <= args.holdetid <= 25:
        parser.error('--holdetid må være mellom 0,2 og 25 s (fristen er 30 s).')
    if os.geteuid() == 0:
        parser.error('Kjør som vanlig bruker, ikke root.')
    try:
        if args.liste:
            for number, (name, text, _) in CANDIDATES.items():
                print(f'{number}  {name:<13} {text}')
            print('Kandidat 5 er det appen bruker som standard; 1–4 er én enkelt skriving.')
            print(f'Hver kandidat tar ca. {args.holdetid + 5:.0f} s. Kommando: python3 {sys.argv[0]} --kandidat N')
        elif args.bekreft:
            confirm(args.socket)
        elif args.kandidat:
            name, base = run_candidate(args.socket, args.kandidat, args.holdetid)
            print(question(name, base))
            print(f'Når lyset er som før: python3 {sys.argv[0]} --bekreft')
        else:
            answers = {}
            for number in CANDIDATES:
                name, base = run_candidate(args.socket, number, args.holdetid)
                answers[name] = input(question(name, base) + '\n> ').strip()
                if input('Viser kortet utgangslyset igjen? [j/n] ').strip().lower() != 'j':
                    print('Stopper. Kontroller kortet, og kjør --bekreft når lyset er som før.')
                    break
                confirm(args.socket)
            print(json.dumps(answers, ensure_ascii=False, indent=1))
    except (Feil, OSError, ValueError) as error:
        print(f'rgb-av-prove: {error}', file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        # Forbindelsen lukkes; hjelperen ruller selv tilbake til utgangslyset.
        print('\nrgb-av-prove: avbrutt. Hjelperen ruller tilbake til utgangslyset av seg selv; '
              f'se på kortet, og kjør python3 {sys.argv[0]} --bekreft når lyset er som før.',
              file=sys.stderr)
        return 130
    return 0


if __name__ == '__main__':
    sys.exit(main())
