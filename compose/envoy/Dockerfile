FROM envoyproxy/envoy-dev:latest
COPY envoy.yaml /etc/envoy/envoy.yaml
COPY envoy_config.yaml /etc/envoy/envoy_config.yaml
# FROM istio/proxyv2:1.6.1
ENTRYPOINT /usr/local/bin/envoy -c /etc/envoy/envoy.yaml  --service-cluster proxy --log-level debug --service-node 'proxy'
