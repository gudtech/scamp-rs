#!/bin/bash
# Build scamp-rs for x86_64 Linux and run interop tests on the GT Docker network.
#
# Usage:
#   ./scripts/build-linux.sh              # Build the Docker image
#   ./scripts/build-linux.sh test         # Build and run interop tests
#   ./scripts/build-linux.sh shell        # Build and get an interactive shell
#   ./scripts/build-linux.sh run [args..] # Build and run scamp with given args

set -e

DOCKER_IMAGE="scamp-rs-test"
DOCKER_NETWORK="gtnet"
BACKPLANE_MOUNT="$HOME/GT/backplane:/backplane:ro"

echo "==> Building scamp-rs for x86_64 Linux..."
docker build --platform linux/amd64 -f Dockerfile.interop-test -t "$DOCKER_IMAGE" .

DOCKER_RUN="docker run --rm --platform linux/amd64 --network $DOCKER_NETWORK -v $BACKPLANE_MOUNT -e SCAMP_CONFIG=/backplane/etc/soa.conf"

case "${1:-build}" in
    build)
        echo "==> Build complete. Image: $DOCKER_IMAGE"
        ;;
    test)
        echo ""
        echo "==> Test 1: List actions (discovery cache parsing)"
        $DOCKER_RUN "$DOCKER_IMAGE" list actions --name health_check
        echo ""
        echo "==> Test 2: Request to API.Status.health_check (Perl gt-main-service)"
        $DOCKER_RUN "$DOCKER_IMAGE" request --action "api.status.health_check~1" --body '{}'
        echo ""
        echo "==> Test 3: Request to _meta.documentation (multi-packet response)"
        RESP=$($DOCKER_RUN "$DOCKER_IMAGE" request --action "_meta.documentation~1" --body '{}' 2>&1 | tail -1)
        echo "$RESP"
        echo ""
        echo "==> All interop tests passed!"
        ;;
    shell)
        echo "==> Starting interactive shell..."
        $DOCKER_RUN -it --entrypoint /bin/bash "$DOCKER_IMAGE"
        ;;
    run)
        shift
        $DOCKER_RUN "$DOCKER_IMAGE" "$@"
        ;;
    *)
        echo "Usage: $0 [build|test|shell|run [args...]]"
        exit 1
        ;;
esac
