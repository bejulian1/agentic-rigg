"""Hent og installer systemkomponentene for én bestemt utgivelse (kjøres som root).

Appen starter dette med `pkexec python3 -I -S -c <denne fila> VERSJON BYGG
[nvml|rgb PCI UUID]` når brukeren slår på skjermkortstyring eller kortlys og
komponentene mangler eller er utdaterte. Med domene aktiveres tilgangen rett
etter installasjonen med de faste, nettopp installerte skriptene, så ett
passord holder. Ingenting fra brukerens
hjemmemappe brukes: arkivet lastes ned fra GitHub-utgivelsen med samme versjon
som appen, inn i en ny rot-eid mappe, og kontrolleres mot SHA-256-summen
GitHub publiserer for akkurat den filen (samme sum som står på utgivelsessiden).
Først da hentes `system-install` ut og kjøres med `python3 -I -S`, som
kontrollerer summen på nytt og installerer bare navngitte filer.

Uten nett finnes terminalkommandoen (`system-bootstrap.command`) med
arkiv og sum oppgitt for hånd.
"""
import hashlib
import http.client
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.parse
import urllib.request

REPO = 'bejulian1/agentic-rigg'
API = 'https://api.github.com/repos/' + REPO + '/releases/tags/v{version}'
DOWNLOAD = 'https://github.com/' + REPO + '/releases/download/v{version}/{name}'
ASSET = 'agentic-rigg-{version}-build{build}-ubuntu26.04-amd64.tar.gz'
MAX_JSON = 1 << 20
MAX_ARCHIVE = 64 << 20
HEADERS = {'Accept': 'application/vnd.github+json', 'User-Agent': 'agentic-rigg-system-install'}
STAGING = '/var/tmp'
INSTALLER = ['/usr/bin/python3', '-I', '-S']
ACTIVATE = {'nvml': ['/usr/local/libexec/agentic-rigg/aktiver.sh'],
            'rgb': ['/usr/local/libexec/agentic-rigg-rgb-v2/rgb-aktiver.sh']}
# RGB-aktiveringen krever at brukeren har sett på kortet; appen spør før pkexec.
ACTIVATE_FLAGS = {'nvml': [], 'rgb': ['--static-confirmed']}
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


def installer_from(archive):
    with tarfile.open(archive, 'r:gz') as tar:
        members = [m for m in tar if m.isfile() and re.fullmatch(r'[^/]+/data/hjelper/system-install', m.name)]
        if len(members) != 1 or members[0].size > 1 << 20:
            fail('Arkivet har ikke ett gyldig installasjonsprogram.')
        return tar.extractfile(members[0]).read()


def activation(argv):
    """`[]` eller `[domene, pci, uuid]` → kommandoen og miljøet, eller None."""
    if not argv:
        return None
    if len(argv) != 3 or argv[0] not in ACTIVATE:
        fail('Forventet VERSJON BYGG [nvml|rgb PCI UUID].')
    domain, pci, uuid = argv[0], argv[1].lower(), argv[2]
    if not re.fullmatch(PCI, pci, re.ASCII) or not re.fullmatch(UUID, uuid, re.ASCII):
        fail('Aktivering krever full PCI-adresse og GPU-UUID.')
    uid = os.environ.get('PKEXEC_UID', '')
    if not re.fullmatch(r'[1-9][0-9]{0,9}', uid, re.ASCII):
        fail('Fant ikke brukeren bak administratorforespørselen.')
    command = [*ACTIVATE[domain], '--pci', pci, '--uuid', uuid, *ACTIVATE_FLAGS[domain]]
    return command, {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'PKEXEC_UID': uid}


def main(argv):
    if len(argv) not in (2, 5) or not re.fullmatch(r'[0-9]{1,4}\.[0-9]{1,4}\.[0-9]{1,4}', argv[0]) or not re.fullmatch(r'[1-9][0-9]{0,8}', argv[1]):
        fail('Forventet VERSJON BYGG [nvml|rgb PCI UUID].')
    if os.geteuid() != 0:
        fail('Systeminstallasjon krever administrator.')
    version, build = argv[:2]
    then = activation(argv[2:])
    name = ASSET.format(version=version, build=build)
    digest = published_digest(version, name)
    os.umask(0o077)
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
            out.write(installer_from(archive))
        result = subprocess.run([*INSTALLER, installer, archive, digest], stdin=subprocess.DEVNULL,
                                env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}, check=False)
        if result.returncode or then is None:
            return result.returncode
    finally:
        shutil.rmtree(temp, ignore_errors=True)
    command, env = then
    return subprocess.run(command, stdin=subprocess.DEVNULL, env=env, check=False).returncode


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except (*NET_ERRORS, ValueError, tarfile.TarError) as error:
        sys.exit(str(error))
