MKFILE_PATH := $(abspath $(lastword $(MAKEFILE_LIST)))
PROJECT_PATH := $(patsubst %/,%,$(dir $(MKFILE_PATH)))
COMPOSEFILE := $(PROJECT_PATH)/compose/docker-compose.yaml
DOCKER_COMPOSE := docker-compose -f $(COMPOSEFILE)
OPEN_APP ?= xdg-open

.PHONY: fetch-protos doc help up up-container up-local stop status top kill \
	down proxy-info proxy ingress-helper ingress-url ingress-open \
	ingress-admin-url ingress-admin-open curl


.PHONY: fetch_protos
fetch_protos: ##  Fetch protobuf files
	$(Q) git submodule update --init --recursive

.PHONY: update_protos
update_protos: ##  Update Protobuf files
	$(Q) git submodule update --remote --merge

.PHONY: wasm_build
wasm_build: ## Build wasm filter
	$(Q) cargo build --target=wasm32-unknown-unknown --lib --manifest-path $(PROJECT_PATH)/wasm_filter/Cargo.toml
	$(Q) cp -fv $(PROJECT_PATH)/wasm_filter/target/wasm32-unknown-unknown/debug/filter.wasm $(PROJECT_PATH)/static/

.PHONY: auth-build
auth-build: ## Build 3scale auth wasm filter
	$(MAKE) -C $(PROJECT_PATH)/threescale-wasm-auth build
	mkdir -p $(PROJECT_PATH)/compose/control-plane/static
	cp -fv $(PROJECT_PATH)/threescale-wasm-auth/compose/wasm/threescale_wasm_auth.wasm $(PROJECT_PATH)/static/

.PHONY: auth-build-clean
auth-build-clean: ## Build 3scale auth wasm filter
	rm -f $(PROJECT_PATH)/compose/control-plane/static/threescale_wasm_auth.wasm
	$(MAKE) -C $(PROJECT_PATH)/threescale-wasm-auth clean

doc: ## open project documentation
	$(Q) cargo doc --open


up-container: export CONTROL_PLANE_LOCAL=control-plane-alt
up-container: export CONTROL_PLANE_DOCKER=control-plane-main
up-container: # Launch docker-compose with the control-plane as a container
	@echo "Launch mode: control-plane in container (use LOCAL_CP=y to use localhost)"
	$(DOCKER_COMPOSE) up

up-local: export CONTROL_PLANE_LOCAL=control-plane-main
up-local: export CONTROL_PLANE_DOCKER=control-plane-alt
up-local: # Launch docker-compose with the control-plane as a (pre-existing) local process
	@echo "Launch mode: control-plane in localhost (use LOCAL_CP=n to use a container)"
	$(DOCKER_COMPOSE) up

up-deps =

ifeq ($(LOCAL_CP),y)
	up-deps = up-local
else
	up-deps = up-container
endif

up: $(up-deps) ## Start docker-compose containers

stop: ## Stop docker-compose containers
	$(DOCKER_COMPOSE) stop

.PHONY: build
build: ## Build containers
	$(DOCKER_COMPOSE) build

status: ## Status of docker-compose containers
	$(DOCKER_COMPOSE) ps

top: ## Show runtime information about docker-compose containers
	$(DOCKER_COMPOSE) top

kill: ## Force-stop docker-compose containers
	$(DOCKER_COMPOSE) kill

down: ## Stop and remove containers and other docker-compose resources
	$(DOCKER_COMPOSE) down

proxy-info: export INDEX?=1
proxy-info: ## Obtain the local host address and port for a service (use SERVICE, PORT and optionally INDEX)
	$(DOCKER_COMPOSE) port --index $(INDEX) $(SERVICE) $(PORT)

proxy-url: export INDEX?=1
proxy-url: export SCHEME?=http
proxy-url: ## Obtain a URL for the given service (use SERVICE, PORT and optionally INDEX)
	$(DOCKER_COMPOSE) port --index $(INDEX) $(SERVICE) $(PORT)

proxy: export INDEX?=1
proxy: export SCHEME?=http
proxy: LOCALURL=$(shell $(DOCKER_COMPOSE) port --index $(INDEX) $(SERVICE) $(PORT))
proxy: ## Open service and port in a browser (same as proxy-info, but optionally define SCHEME and OPEN_APP)
	$(OPEN_APP) $(SCHEME)://$(LOCALURL)

ingress-helper: export SERVICE?=ingress
ingress-helper: export PORT?=80
ingress-helper: export TARGET?=proxy-url
ingress-helper:
	$(MAKE) $(TARGET)

ingress-url: ## Show the ingress URL
	$(MAKE) ingress-helper

ingress-open: export TARGET?=proxy
ingress-open: ## Open the ingress URL
	$(MAKE) ingress-helper

ingress-admin-url: export PORT?=8001
ingress-admin-url: ## Show the ingress admin URL
	$(MAKE) ingress-helper

ingress-admin-open: export PORT?=8001
ingress-admin-open: export TARGET?=proxy
ingress-admin-open: ## Open the ingress admin URL
	$(MAKE) ingress-helper

curl: export SCHEME?=http
curl: export SERVICE?=ingress
curl: export INDEX?=1
curl: export PORT?=80
curl: export HOST?=web.app
curl: ## Perform a request to a specific service (default ingress:80 with Host: web.app)
	curl -vvv -H "Host: $(HOST)" "$(SCHEME)://$$($(DOCKER_COMPOSE) port --index $(INDEX) $(SERVICE) $(PORT))/"

# Check http://marmelab.com/blog/2016/02/29/auto-documented-makefile.html
help: ## Print this help
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
