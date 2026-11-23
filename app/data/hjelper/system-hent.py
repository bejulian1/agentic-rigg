"""Hent og installer systemkomponentene for én bestemt utgivelse (kjøres som root).

Appen starter dette med `pkexec python3 -I -S -c <denne fila> VERSJON BYGG
FESTET [nvml|rgb PCI UUID]` når brukeren slår på skjermkortstyring eller
kortlys og komponentene mangler eller er utdaterte. Med domene aktiveres
tilgangen rett etter installasjonen med de faste, nettopp installerte
skriptene, så ett passord holder.

Tillitsmodell. Skriptet og argumentene kommer fra brukerens app under
~/.local, så de er ikke sikrere enn brukerkontoen som starter pkexec. Root
leser ingen filer fra hjemmemappa: arkivet lastes ned fra GitHub-utgivelsen
med samme versjon og bygg som appen, inn i en ny rot-eid mappe, og
kontrolleres mot SHA-256-summen GitHub oppgir for akkurat den filen. Det
beviser at filen er den som ligger på utgivelsen nå, ikke hvem som la den
der. Derfor kontrolleres også identiteten (én rotmappe, `version.json` med
nøyaktig samme versjon og bygg). FESTET er summene fra utgivelsesarkivet
appen selv ble installert fra (`SYSTEMFILER.sha256`, lagt der av
`install.sh`). Da må hvert systemrelevant medlem (`bin/*`, `version.json`
og `data/hjelper/*`) ha nøyaktig de summene, så en senere utskiftet
utgivelse avvises. Er appen bygd fra kilde, er FESTET `-`, og bare
GitHub-summen og identiteten kontrolleres. En signert `SHA256SUMS` med
en innebygd offentlig nøkkel krever en egen signeringsnøkkel og er ikke
innført ennå (se docs/produksjon/rgb-hjelper.md).

Først etter kontrollene hentes `system-install` ut og kjøres med
`python3 -I -S`, som kontrollerer summen på nytt og installerer bare
navngitte filer.

Låsing. Tillatelsene leses før nettet brukes og leses på nytt etter
nedlastingen. Har noe endret seg, stopper alt før en eneste fil er
byttet. RGB avsluttes med de gamle, installerte skriptene, som tar
`/run/agentic-rigg-system.lock` selv. Deretter holder dette skriptet den
felles låsen gjennom ny kontroll, installasjon, gjenaktivering av
GPU-styringen og den valgte aktiveringen. De nyinstallerte skriptene får
låsen som et arvet filhåndtak (`AGENTIC_RIGG_SYSTEM_LOCK_FD`), så ingen
annen administratorhandling kommer imellom og en tilbakekalt tillatelse
aldri gis tilbake.

Installasjonen bytter hjelperne og fjerner derfor aktive tillatelser.
En GPU-tillatelse for samme bruker aktiveres igjen for samme kort etterpå.

GPU-styring (og en oppdatering uten aktivering) rører aldri en aktiv
RGB-del eller en RGB-journal: da installeres bare NVML-delen og
systeminstallasjonen (`--keep-rgb`), og RGB-hjelperen, tillatelsen,
lyset og en eventuell uavklart økt blir stående som før. Appen får én
linje på stdout (`RGB_KEPT_PREFIX`) og viser at RGB-avklaringen gjøres
separat på siden RGB-lys. En uavklart RGB-økt kan dermed aldri stenge
brukeren ute fra GPU-styring.

Ber brukeren om RGB og er GPU-delen fra nøyaktig denne utgivelsen og aktiv,
installeres bare RGB-delen (`--keep-nvml`): GPU-hjelperen stoppes ikke, så
beholdte GPU-verdier står. Ellers gis GPU-tillatelsen tilbake etterpå.

Ber brukeren om RGB, slås en eksisterende RGB-tilgang av med det
installerte `rgb-deaktiver.sh` (lyset og utgangspunktet beholdes). Har den
gamle RGB-hjelperen en uavklart økt (journal), skrives det lagrede
utgangspunktet tilbake først. Så sender skriptet avklaringsmålet til appen
(én linje på stdout) og venter på at brukeren ser på kortet og bekrefter
nøyaktig den gjenopprettingsidentiteten (én linje på stdin). Først da
avklares journalen. Uten den bekreftelsen beholdes journalen og ingenting
byttes.

Uten nett finnes terminalkommandoen (`system-bootstrap.command`) med
arkiv og sum oppgitt for hånd.
"""
import fcntl
import hashlib
import http.client
import json
import os
import re
import select
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

# Visningsnavnet i arkivets `version.json`. Appen het «Agentic Rigg» til og
# med 1.6.0 (bygg 32), så utgivelser fra den tiden godtas med det gamle
# navnet. Senere bygg må oppgi det nye. Repo, arkivnavn, stier og enheter er
# tekniske identifikatorer og er uendret.
NAME = 'Agentic HW Monitor'
OLD_NAME = 'Agentic Rigg'
OLD_NAME_LAST_BUILD = 32
REPO = 'bejulian1/agentic-rigg'
API = 'https://api.github.com/repos/' + REPO + '/releases/tags/v{version}'
DOWNLOAD = 'https://github.com/' + REPO + '/releases/download/v{version}/{name}'
ASSET = 'agentic-rigg-{version}-build{build}-ubuntu26.04-amd64.tar.gz'
MAX_JSON = 1 << 20
MAX_ARCHIVE = 64 << 20
MAX_UNPACKED = 256 << 20
HEADERS = {'Accept': 'application/vnd.github+json', 'User-Agent': 'agentic-rigg-system-install'}
STAGING = '/var/tmp'
MARK = '/usr/local/libexec/agentic-rigg-system/revision'
INSTALLER = ['/usr/bin/python3', '-I', '-S']
ACTIVATE = {'nvml': ['/usr/local/libexec/agentic-rigg/aktiver.sh'],
            'rgb': ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-aktiver.sh']}
# RGB-aktiveringen krever at brukeren har sett på kortet; appen spør før pkexec.
ACTIVATE_FLAGS = {'nvml': [], 'rgb': ['--static-confirmed']}
RGB_DEACTIVATE = ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-deaktiver.sh']
RGB_RESTORE = ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-gjenopprett.sh']
RGB_RESOLVE = ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-avklar.sh']
RGB_JOURNAL = '/var/lib/agentic-rigg-rgb/journal.json'
CONFIG = {'nvml': '/etc/agentic-rigg/helper.json', 'rgb': '/etc/agentic-rigg/rgb.json'}
OWNER = 0
LOCK = '/run/agentic-rigg-system.lock'
LOCK_FD_ENV = 'AGENTIC_RIGG_SYSTEM_LOCK_FD'
# Linjen appen ser etter på stdout, og hvor lenge brukeren har på seg.
CONFIRM_PREFIX = 'AGENTIC-RIGG-RGB-AVKLAR '
CONFIRM_TIMEOUT = 900
# Linjen appen får når RGB-delen ble stående urørt (se `--keep-rgb`).
RGB_KEPT_PREFIX = 'AGENTIC-RIGG-RGB-BEHOLDT '
# Medlemmene som installeres eller kjøres som root (og appens binærer).
SYSTEM_PATH = r'bin/[A-Za-z0-9._-]{1,64}|version\.json|data/hjelper/[A-Za-z0-9._-]{1,64}'
MAX_PINS = 64
CHANGED = 'Tillatelsene endret seg mens oppdateringen ble forberedt. Ingen systemfiler er endret. Prøv igjen.'
PCI = r'[0-9a-f]{4}:[0-9a-f]{2}:[01][0-9a-f]\.[0-7]'
UUID = r'GPU-[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}'


NET_ERRORS = (OSError, http.client.HTTPException)


def fail(text):
    raise SystemExit(text)


def trusted_host(url, hosts):
    parts = urllib.parse.urlsplit(url)
    host = (parts.hostname or '').lower()
    return parts.scheme == 'https' and any(host == h or (h.startswith('.') and host.endswith(h)) for h in hosts)


class Redirects(urllib.request.HTTPRedirectHandler):
    """Bare HTTPS-videresending til de oppgitte vertene; ingen for API-et."""

    def __init__(self, hosts):
        super().__init__()
        self.hosts = hosts

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        if not trusted_host(newurl, self.hosts):
            fail('Nedlastingen ble sendt videre til en ukjent adresse.')
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def open_url(request, hosts):
    return urllib.request.build_opener(Redirects(hosts)).open(request, timeout=60)


def fetch(url, limit, out=None, hosts=()):
    """Les `url` over HTTPS (sertifikatet kontrolleres). `hosts` er vertene
    en videresending kan gå til. Med `out` skrives innholdet dit og SHA-256
    returneres; ellers returneres bytene."""
    request = urllib.request.Request(url, headers=HEADERS)
    digest = hashlib.sha256()
    data = bytearray()
    size = 0
    with open_url(request, hosts) as response:
        if not trusted_host(response.geturl(), ('api.github.com', 'github.com', *hosts)):
            fail('Nedlastingen ble sendt videre til en usikker adresse.')
        while chunk := response.read(1 << 16):
            size += len(chunk)
            if size > limit:
                fail('Nedlastingen er større enn forventet.')
            if out is None:
                data += chunk
            else:
                digest.update(chunk)
                out.write(chunk)
    return bytes(data) if out is None else digest.hexdigest()


def published_digest(version, name):
    try:
        release = json.loads(fetch(API.format(version=version), MAX_JSON))
    except urllib.error.HTTPError as error:
        if error.code == 404:
            fail(f'Utgivelsen v{version} finnes ikke på GitHub.')
        fail(f'Fikk ikke kontakt med GitHub: HTTP {error.code}. Prøv igjen senere, eller bruk terminalkommandoen.')
    except (*NET_ERRORS, ValueError) as error:
        fail(f'Fikk ikke kontakt med GitHub: {error}. Sjekk nettet, eller bruk terminalkommandoen.')
    if not isinstance(release, dict) or not isinstance(release.get('assets'), list):
        fail('Uventet svar fra GitHub.')
    assets = [asset for asset in release['assets'] if isinstance(asset, dict) and asset.get('name') == name]
    if len(assets) != 1:
        fail(f'Utgivelsen v{version} på GitHub har ikke {name}.')
    match = re.fullmatch(r'sha256:([0-9a-f]{64})', str(assets[0].get('digest') or ''))
    if not match:
        fail('GitHub oppgir ingen SHA-256 for arkivet.')
    return match[1]


def pinned(text):
    """`-` → None, ellers {sti: sha256} fra `SYSTEMFILER.sha256` (sha256sum-format)."""
    if text == '-':
        return None
    lines = text.split('\n')
    if lines[-1] == '':
        lines.pop()
    if not 0 < len(lines) <= MAX_PINS or len(text) > 16384:
        fail('Ugyldige festede summer fra appen.')
    pins = {}
    for line in lines:
        match = re.fullmatch(r'([0-9a-f]{64})  (' + SYSTEM_PATH + ')', line, re.ASCII)
        if not match or match[2] in pins:
            fail('Ugyldige festede summer fra appen.')
        pins[match[2]] = match[1]
    if not {'version.json', 'bin/agentic-rigg-helper', 'data/hjelper/system-install'} <= pins.keys():
        fail('De festede summene mangler systemfiler.')
    return pins


def known_name(name, build):
    """Er `name` appens visningsnavn for en utgivelse med dette bygget?"""
    return name == NAME or (name == OLD_NAME and build <= OLD_NAME_LAST_BUILD)


def verified_installer(archive, version, build, pins):
    """Installasjonsprogrammet fra et arkiv med riktig identitet.

    Arkivet må ha én rotmappe `agentic-rigg-VERSJON` med bare vanlige filer,
    og `version.json` må oppgi appens navn og nøyaktig samme versjon og bygg
    (se `known_name` for navnet før og etter navnebyttet). Med `pins`
    må de systemrelevante medlemmene være nøyaktig de festede filene.
    """
    root = f'agentic-rigg-{version}'
    digests, kept, unpacked = {}, {}, 0
    with tarfile.open(archive, 'r:gz') as tar:
        for member in tar:
            top, slash, relative = member.name.partition('/')
            parts = relative.split('/')
            if (top != root or not slash or not member.isfile()
                    or any(part in ('', '.', '..') for part in parts)):
                fail('Arkivet har en uventet struktur. Ingen systemfiler endret.')
            # Avgrenset utpakking: en liten gzip kan ellers bli gigabyte.
            unpacked += member.size
            if unpacked > MAX_UNPACKED:
                fail('Arkivet er større enn forventet utpakket. Ingen systemfiler endret.')
            if not re.fullmatch(SYSTEM_PATH, relative, re.ASCII):
                continue
            if relative in digests:
                fail(f'Arkivet har et ugyldig systemmedlem: {relative}')
            digest = hashlib.sha256()
            limit = {'version.json': 4096, 'data/hjelper/system-install': 1 << 20}.get(relative)
            data = bytearray()
            with tar.extractfile(member) as stream:
                while chunk := stream.read(1 << 16):
                    digest.update(chunk)
                    if limit is not None and len(data) <= limit:
                        data += chunk
            digests[relative] = digest.hexdigest()
            if limit is not None and len(data) <= limit:
                kept[relative] = bytes(data)
    try:
        identity = json.loads(kept.get('version.json', b''))
    except ValueError:
        identity = None
    if (not isinstance(identity, dict) or not known_name(identity.get('name'), int(build))
            or identity.get('version') != version or type(identity.get('build')) is not int
            or identity['build'] != int(build)):
        fail(f'Arkivet er ikke {NAME} {version} bygg {build}. Ingen systemfiler endret.')
    installer = kept.get('data/hjelper/system-install')
    if installer is None:
        fail('Arkivet har ikke ett gyldig installasjonsprogram.')
    if pins is not None:
        if digests != pins:
            changed = sorted(set(digests) ^ set(pins) | {p for p in digests.keys() & pins.keys() if digests[p] != pins[p]})
            fail('Arkivet samsvarer ikke med utgivelsen appen ble installert fra '
                 f'({", ".join(changed[:3])}). Ingen systemfiler endret.')
    return installer


def pkexec_uid():
    uid = os.environ.get('PKEXEC_UID', '')
    if not re.fullmatch(r'[1-9][0-9]{0,9}', uid, re.ASCII):
        fail('Fant ikke brukeren bak administratorforespørselen.')
    return uid


def env(uid, lock=None):
    values = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'PKEXEC_UID': uid}
    if lock is not None:
        values[LOCK_FD_ENV] = str(lock)
    return values


def activation(argv):
    """`[]` eller `[domene, pci, uuid]` → aktiveringskommandoen, eller None."""
    if not argv:
        return None
    if len(argv) != 3 or argv[0] not in ACTIVATE:
        fail('Forventet VERSJON BYGG [nvml|rgb PCI UUID].')
    domain, pci, uuid = argv[0], argv[1].lower(), argv[2]
    if not re.fullmatch(PCI, pci, re.ASCII) or not re.fullmatch(UUID, uuid, re.ASCII):
        fail('Aktivering krever full PCI-adresse og GPU-UUID.')
    pkexec_uid()
    return [*ACTIVATE[domain], '--pci', pci, '--uuid', uuid, *ACTIVATE_FLAGS[domain]]


def installed_build():
    """Bygget i den betrodde systemrevisjonen (`2:BYGG`), eller None."""
    try:
        fd = os.open(MARK, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except FileNotFoundError:
        return None
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != OWNER or info.st_mode & 0o022:
            return None
        found = re.fullmatch(rb'2:([0-9]{1,10})', stream.read(65).rstrip())
    return int(found[1]) if found else None


def reject_downgrade(build):
    try:
        fd = os.open(MARK, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except FileNotFoundError:
        return
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != OWNER or info.st_mode & 0o022:
            fail(f'Utrygg systemrevisjon: {MARK}')
        revision = stream.read(65)
    if len(revision) > 64:
        fail(f'Ugyldig systemrevisjon: {MARK}')
    installed = re.fullmatch(rb'2:([0-9]+)', revision.rstrip())
    if installed and int(installed[1]) > int(build):
        fail(f'Kan ikke nedgradere systemkomponentene fra bygg {int(installed[1])} til {build}.')


def root_json(path):
    """En root-eid, ikke gruppe-/allment skrivbar tillatelse, eller None."""
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except FileNotFoundError:
        return None
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != OWNER or info.st_mode & 0o022:
            fail(f'Utrygg tillatelsesfil: {path}')
        raw = stream.read(16385)
    if len(raw) > 16384:
        fail(f'For stor tillatelsesfil: {path}')
    try:
        data = json.loads(raw)
    except ValueError:
        fail(f'Ugyldig tillatelsesfil: {path}')
    return data if isinstance(data, dict) else fail(f'Ugyldig tillatelsesfil: {path}')


def authorization(domain, uid):
    """(tillatelsen, (pci, uuid)) for en aktiv tillatelse i `domain` som tilhører `uid`."""
    data = root_json(CONFIG[domain])
    if data is None:
        return None
    if str(data.get('uid')) != uid:
        fail('Den aktive tillatelsen gjelder en annen bruker. Slå den av først.')
    if data.get('enabled') is not True:
        fail('Den lagrede tillatelsen bruker et eldre oppsett eller er ikke aktiv. Fjern den gamle tillatelsen før installasjon.')
    card = data.get('binding') if domain == 'rgb' else data
    if not isinstance(card, dict):
        fail('Den aktive tillatelsen har ugyldig kortbinding.')
    pci, uuid = str(card.get('pci', '')).lower(), str(card.get('uuid', ''))
    if not re.fullmatch(PCI, pci, re.ASCII) or not re.fullmatch(UUID, uuid, re.ASCII):
        fail('Den aktive tillatelsen har ugyldig kortbinding.')
    return data, (pci, uuid)


def snapshot(uid):
    """Alt oppdateringen bygger på: begge tillatelsene og om RGB har en journal."""
    return authorization('nvml', uid), authorization('rgb', uid), os.path.lexists(RGB_JOURNAL)


def card(auth):
    return None if auth is None else auth[1]


class SystemLock:
    """Den felles låsen for administratorhandlinger, holdt til `release`."""

    def __init__(self):
        self.fd = None

    def acquire(self):
        fd = os.open(LOCK, os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != OWNER:
                fail(f'Utrygg låsefil: {LOCK}')
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                fail('En annen administratorhandling pågår. Ingen systemfiler er byttet. Vent og prøv igjen.')
        except BaseException:
            os.close(fd)
            raise
        self.fd = fd
        return fd

    def release(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


def run(command, uid, lock=None):
    fds = () if lock is None else (lock,)
    return subprocess.run(command, stdin=subprocess.DEVNULL, env=env(uid, lock), pass_fds=fds,
                          check=False).returncode


def confirmation(timeout=CONFIRM_TIMEOUT, fd=0):
    """Én linje fra appen på stdin (`fd`) innen `timeout`, eller None (EOF, tidsavbrudd, feil)."""
    deadline = time.monotonic() + timeout
    data = b''
    while b'\n' not in data and len(data) <= 64:
        left = deadline - time.monotonic()
        if left <= 0:
            return None
        try:
            ready, _, _ = select.select([fd], [], [], left)
            if not ready:
                return None
            chunk = os.read(fd, 65 - len(data))
        except (OSError, ValueError):
            return None
        if not chunk:
            break
        data += chunk
    line, newline, _ = data.partition(b'\n')
    try:
        return line.decode('ascii') if newline else None
    except UnicodeDecodeError:
        return None


def settle_rgb(bound, uid):
    """Avslutt en uavklart RGB-økt for kortet `bound` med de installerte skriptene.

    Gjenopprettingen skriver det lagrede utgangspunktet og krever at
    avlesningen samsvarer. Avlesningen kan ikke bekrefte effektmodusen, så
    brukeren må se det gjenopprettede lyset: appen får avklaringsmålet og
    svarer med gjenopprettingsidentiteten først når brukeren har bekreftet
    det. Avklaringen leser på nytt før journalen fjernes. Feiler et trinn,
    eller kommer ingen bekreftelse, beholdes journalen.
    """
    ids = ['--pci', bound[0], '--uuid', bound[1]]
    if run([*RGB_RESTORE, *ids, '--restore-original'], uid):
        print('RGB: Det lagrede lyset kunne ikke gjenopprettes. Journalen er beholdt; ingenting er byttet.', file=sys.stderr)
        return False
    after = 'RGB: Kortet er satt til det lagrede lyset, men økten ble ikke avsluttet. Velg «Avklar tidligere økt».'
    inspect = subprocess.run([*RGB_RESOLVE, *ids, '--inspect'], stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, env=env(uid), check=False)
    if inspect.returncode:
        print(after, file=sys.stderr)
        return False
    try:
        report = json.loads(inspect.stdout or b'null')
    except ValueError:
        report = None
    if report == {}:
        return True
    recovery = report.get('recoveryId') if isinstance(report, dict) else None
    if (not isinstance(recovery, str) or not re.fullmatch(r'[0-9a-f]{32}', recovery, re.ASCII)
            or not isinstance(report.get('expected'), dict)):
        fail('RGB-hjelperen ga et ugyldig avklaringsmål. Journalen er beholdt.')
    target = {'pci': bound[0], 'uuid': bound[1], 'expected': report['expected'], 'recoveryId': recovery}
    print(CONFIRM_PREFIX + json.dumps(target, separators=(',', ':')), flush=True)
    if confirmation() != recovery:
        print('RGB: Kortet er satt til det lagrede lyset, men det gjenopprettede lyset ble ikke bekreftet. '
              'Journalen er beholdt; ingenting er byttet. Kontroller lyset og velg «Avklar tidligere økt».', file=sys.stderr)
        return False
    if run([*RGB_RESOLVE, *ids, '--static-confirmed', '--recovery-id', recovery], uid):
        print(after, file=sys.stderr)
        return False
    return True


def main(argv):
    if (len(argv) not in (3, 6) or not re.fullmatch(r'[0-9]{1,4}\.[0-9]{1,4}\.[0-9]{1,4}', argv[0])
            or not re.fullmatch(r'[1-9][0-9]{0,8}', argv[1])):
        fail('Forventet VERSJON BYGG FESTET [nvml|rgb PCI UUID].')
    if os.geteuid() != 0:
        fail('Systeminstallasjon krever administrator.')
    version, build = argv[:2]
    reject_downgrade(build)
    pins = pinned(argv[2])
    then = activation(argv[3:])
    uid = pkexec_uid()
    # Leses før nettet brukes, så en fremmed tillatelse stopper alt tidlig.
    before = snapshot(uid)
    nvml, rgb, journal = before
    # Brukeren valgte RGB for dette kortet (bare RGB-aktivering).
    chosen = (argv[4].lower(), argv[5]) if then and argv[3] == 'rgb' else None
    # GPU-styring rører ikke en aktiv eller uavklart RGB-del (se over).
    keep_rgb = chosen is None and (rgb is not None or journal)
    # RGB-lys rører ikke en aktiv GPU-hjelper fra nøyaktig denne utgivelsen:
    # da byttes bare RGB-delen, og GPU-verdier brukeren har beholdt, står.
    keep_nvml = chosen is not None and nvml is not None and installed_build() == int(build)
    unsettled = rgb is not None and journal and not keep_rgb
    if chosen is not None and journal and rgb is None:
        fail('Det finnes en RGB-journal uten RGB-tillatelse. Den kan ikke avklares herfra. '
             'Bruk terminalkommandoen eller kontakt utvikleren; ingen systemfiler er endret.')
    if unsettled and chosen != card(rgb):
        fail(f'En tidligere RGB-økt for kortet {card(rgb)[0]} er ikke avsluttet. Velg det kortet, åpne RGB-lys og trykk «Bruk».')
    name = ASSET.format(version=version, build=build)
    digest = published_digest(version, name)
    os.umask(0o077)
    lock = SystemLock()
    try:
        temp = tempfile.mkdtemp(prefix='rigg.', dir=STAGING)
        try:
            archive = os.path.join(temp, 'release')
            try:
                with open(archive, 'xb') as out:
                    got = fetch(DOWNLOAD.format(version=version, name=name), MAX_ARCHIVE, out,
                                hosts=('github.com', '.githubusercontent.com'))
            except NET_ERRORS as error:
                fail(f'Nedlastingen fra GitHub feilet: {error}')
            if got != digest:
                fail('Kontrollsummen stemmer ikke. Ingen systemfiler endret.')
            installer = os.path.join(temp, 'installer')
            with open(installer, 'xb') as out:
                out.write(verified_installer(archive, version, build, pins))
            reject_downgrade(build)
            # Nedlastingen kan ta et minutt. Ingenting brukes fra det gamle
            # øyeblikksbildet hvis en tillatelse er endret eller trukket tilbake.
            if snapshot(uid) != before:
                fail(CHANGED)
            # Installasjonen nekter mens RGB er aktiv. Den gamle, installerte
            # deaktiveringen beholder lyset og utgangspunktet. En uavklart økt
            # avsluttes først (se `settle_rgb`); feiler det, stopper alt her
            # før noe er byttet. De gamle skriptene tar den felles låsen selv.
            if unsettled and not settle_rgb(card(rgb), uid):
                return 1
            if not keep_rgb and rgb is not None and run([*RGB_DEACTIVATE, '--pci', card(rgb)[0], '--uuid', card(rgb)[1]], uid):
                if unsettled:
                    print('RGB: Økten er avsluttet, men den gamle RGB-tilgangen ble ikke slått av. Trykk «Bruk» igjen.', file=sys.stderr)
                return 1
            # Herfra holdes låsen til den valgte aktiveringen er ferdig.
            fd = lock.acquire()
            reject_downgrade(build)
            nvml_now, rgb_now, journal_now = snapshot(uid)
            if keep_rgb:
                # RGB-delen skal være nøyaktig som før: samme tillatelse og journal.
                if (nvml_now, rgb_now, journal_now) != before:
                    fail(CHANGED)
            elif nvml_now != nvml or rgb_now is not None or journal_now:
                fail(CHANGED + (' RGB-tilgangen er slått av; trykk «Bruk» for å slå den på igjen.' if rgb else ''))
            keep = ['--keep-rgb'] if keep_rgb else ['--keep-nvml'] if keep_nvml else []
            result = subprocess.run([*INSTALLER, installer, *keep, archive, digest], stdin=subprocess.DEVNULL,
                                    env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', LOCK_FD_ENV: str(fd)},
                                    pass_fds=(fd,), check=False)
            if result.returncode:
                return result.returncode
        finally:
            shutil.rmtree(temp, ignore_errors=True)
        if keep_rgb:
            # RGB-lyset, tillatelsen og en eventuell journal er urørt.
            kept = {'pci': card(rgb)[0] if rgb else None, 'unsettled': journal}
            print(RGB_KEPT_PREFIX + json.dumps(kept, separators=(',', ':')), flush=True)
        # Installasjonen fjernet GPU-tillatelsen. Gi den tilbake for samme kort,
        # med mindre brukeren nettopp ba om GPU-styring (da gjør `then` det)
        # eller GPU-delen ble stående (`keep_nvml`).
        if nvml is not None and not keep_nvml and not (then and then[0] == ACTIVATE['nvml'][0]):
            if run([*ACTIVATE['nvml'], '--pci', card(nvml)[0], '--uuid', card(nvml)[1]], uid, fd):
                print('GPU-styringen ble ikke slått på igjen. Slå på bryteren i appen.', file=sys.stderr)
        if then is None:
            return 0
        return run(then, uid, fd)
    finally:
        lock.release()


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except (*NET_ERRORS, ValueError, tarfile.TarError) as error:
        sys.exit(str(error))
