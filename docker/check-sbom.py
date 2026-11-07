#!/usr/bin/env python3
"""Checks the SBOMs of an image for several platforms in an OCI layout, as
`docker buildx build --sbom=true --output type=oci,dest=<file>` writes it: the image of each
platform must come with an SBOM that lists the given packages, e.g. the crates that cargo
auditable embedded in the binary, so that scanners like Trivy or Grype find them in the image.

Usage: docker/check-sbom.py <OCI layout as tar> <package>...
"""

import json
import sys
import tarfile

# How BuildKit marks the manifests of attestations, and the SPDX documents among them.
REFERENCE_TYPE = "vnd.docker.reference.type"
REFERENCE_DIGEST = "vnd.docker.reference.digest"
ATTESTATION = "attestation-manifest"
PREDICATE_TYPE = "in-toto.io/predicate-type"
SPDX = "https://spdx.dev/Document"


def read_layout(path: str) -> tuple[dict[str, str], dict[str, set[str]]]:
    """Returns the platform of each image manifest, and the packages of the SBOMs of each, both
    by the digest of the image manifest."""
    images: dict[str, str] = {}
    sboms: dict[str, set[str]] = {}
    with tarfile.open(path) as tar:
        members = {name.removeprefix("./"): name for name in tar.getnames()}

        def read(name: str):
            file = tar.extractfile(members[name]) if name in members else None
            if file is None:
                raise SystemExit(f"{path} has no file {name}")
            return json.load(file)

        def blob(digest: str):
            algorithm, encoded = digest.split(":", 1)
            return read(f"blobs/{algorithm}/{encoded}")

        pending = list(read("index.json")["manifests"])
        while pending:
            descriptor = pending.pop()
            content = blob(descriptor["digest"])
            if "manifests" in content:
                # An index, e.g. of the image for all platforms.
                pending.extend(content["manifests"])
                continue
            annotations = descriptor.get("annotations", {})
            if annotations.get(REFERENCE_TYPE) != ATTESTATION:
                platform = descriptor.get("platform", {})
                images[descriptor["digest"]] = "/".join(
                    part
                    for part in (
                        platform.get("os"),
                        platform.get("architecture"),
                        platform.get("variant"),
                    )
                    if part
                )
                continue
            for layer in content.get("layers", []):
                if layer.get("annotations", {}).get(PREDICATE_TYPE) == SPDX:
                    statement = blob(layer["digest"])
                    packages = statement.get("predicate", {}).get("packages", [])
                    sboms.setdefault(annotations[REFERENCE_DIGEST], set()).update(
                        package.get("name") for package in packages
                    )
    return images, sboms


def main(path: str, required: list[str]) -> int:
    images, sboms = read_layout(path)
    if not images:
        print(f"{path} holds no image")
        return 1
    failed = False
    for digest, platform in sorted(images.items(), key=lambda item: item[1]):
        packages = sboms.get(digest)
        if packages is None:
            print(f"{platform}: no SBOM")
            failed = True
            continue
        missing = [name for name in required if name not in packages]
        if missing:
            print(f"{platform}: the SBOM lacks {', '.join(missing)}")
            failed = True
        else:
            print(
                f"{platform}: the SBOM lists {len(packages)} packages, "
                f"among them {', '.join(required)}"
            )
    return 1 if failed else 0


if __name__ == "__main__":
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    sys.exit(main(sys.argv[1], sys.argv[2:]))
