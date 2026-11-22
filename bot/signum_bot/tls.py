from __future__ import annotations

import datetime as dt
import ipaddress
import ssl
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID

# iOS не принимает сертификаты со сроком жизни больше 398 дней — выданные после
# сентября 2020 просто отбраковываются, каким бы доверенным ни был корень.
DAYS = 397


def ensure_cert(data_dir: Path, hosts: list[str]) -> tuple[Path, Path]:
    """Готовит самоподписанный сертификат для локальной проверки на iPhone.

    Пересоздаётся, если сменился список адресов или срок подошёл к концу.
    """
    cert_path = data_dir / "local-cert.pem"
    key_path = data_dir / "local-key.pem"
    if cert_path.exists() and key_path.exists() and _matches(cert_path, hosts):
        return cert_path, key_path

    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "Signum Local")])
    now = dt.datetime.now(dt.timezone.utc)
    alt: list[x509.GeneralName] = []
    for host in hosts:
        try:
            alt.append(x509.IPAddress(ipaddress.ip_address(host)))
        except ValueError:
            alt.append(x509.DNSName(host))

    cert = (
        x509.CertificateBuilder()
        .subject_name(name)
        .issuer_name(name)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - dt.timedelta(minutes=5))
        .not_valid_after(now + dt.timedelta(days=DAYS))
        .add_extension(x509.SubjectAlternativeName(alt), critical=False)
        .add_extension(x509.BasicConstraints(ca=True, path_length=None), critical=True)
        .add_extension(
            x509.ExtendedKeyUsage([x509.oid.ExtendedKeyUsageOID.SERVER_AUTH]),
            critical=False,
        )
        .sign(key, hashes.SHA256())
    )
    cert_path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
    key_path.write_bytes(
        key.private_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PrivateFormat.TraditionalOpenSSL,
            encryption_algorithm=serialization.NoEncryption(),
        )
    )
    return cert_path, key_path


def _matches(cert_path: Path, hosts: list[str]) -> bool:
    try:
        cert = x509.load_pem_x509_certificate(cert_path.read_bytes())
        names = cert.extensions.get_extension_for_class(
            x509.SubjectAlternativeName
        ).value
        have = {str(item.value) for item in names}
        fresh = cert.not_valid_after_utc > dt.datetime.now(
            dt.timezone.utc
        ) + dt.timedelta(days=7)
        return fresh and set(hosts) <= have
    except Exception:  # noqa: BLE001 - битый или старый файл проще перевыпустить
        return False


def context(cert_path: Path, key_path: Path) -> ssl.SSLContext:
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(cert_path, key_path)
    return ctx
