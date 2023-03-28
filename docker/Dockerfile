FROM debian:bookworm-slim

EXPOSE 8000
EXPOSE 9443

ARG CONFIG_PATH=/config
ENV UC_CONFIG_HOME=$CONFIG_PATH

RUN mkdir $CONFIG_PATH && chown 10000 $CONFIG_PATH

# for static configuration without driver setup flow:
#ENV UC_HASS_URL=ws://hassio.local:8123/api/websocket
#ENV UC_HASS_TOKEN=OVERRIDE_WITH_YOUR_LONG_LIVED_ACCESS_TOKEN

WORKDIR /app

COPY ./app /app

USER 10000

VOLUME $CONFIG_PATH

CMD ["/app/uc-intg-hass"]

# Labels, see: https://github.com/opencontainers/image-spec/blob/master/annotations.md
ARG BUILD_DATE
ARG VERSION
ARG REVISION
LABEL org.opencontainers.image.created=$BUILD_DATE
LABEL org.opencontainers.image.authors="markus.z@unfoldedcircle.com"
LABEL org.opencontainers.image.url="https://hub.docker.com/r/unfoldedcircle/integration-hass"
LABEL org.opencontainers.image.version=$VERSION
LABEL org.opencontainers.image.revision=$REVISION
LABEL org.opencontainers.image.vendor="Unfolded Circle"
LABEL org.opencontainers.image.title="Unfolded Circle Home Assistant integration"
LABEL org.opencontainers.image.description="Remote Two integration for Home Assistant written in Rust"
