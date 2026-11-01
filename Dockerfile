# syntax=docker/dockerfile:1
# The release image: the static seam binary and nothing else. The binaries
# are built by .github/workflows/release.yml; this file only packages them.
FROM scratch
ARG TARGETARCH
COPY --chmod=0755 dist/${TARGETARCH}/seam /usr/local/bin/seam
WORKDIR /repo
ENTRYPOINT ["/usr/local/bin/seam"]
CMD ["--help"]
