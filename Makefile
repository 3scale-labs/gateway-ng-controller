MKFILE_PATH := $(abspath $(lastword $(MAKEFILE_LIST)))
PROJECT_PATH := $(patsubst %/,%,$(dir $(MKFILE_PATH)))
COMPOSEFILE := $(PROJECT_PATH)/compose/docker-compose.yaml
DOCKER_COMPOSE := docker-compose -f $(COMPOSEFILE)
OPEN_APP ?= xdg-open

.PHONY: fetch-protos doc help up up-container up-local stop status top kill down proxy-info proxy

# Check http://marmelab.com/blog/2016/02/29/auto-documented-makefile.html
fetch_protos: ## Fetch protobuf files
	$(Q) git submodule update --init --recursive

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

proxy: export INDEX?=1
proxy: export SCHEME?=http
proxy: LOCALURL=$(shell $(DOCKER_COMPOSE) port --index $(INDEX) $(SERVICE) $(PORT))
proxy: ## Open service and port in a browser (same as proxy-info, but optionally define SCHEME and OPEN_APP)
	$(OPEN_APP) $(SCHEME)://$(LOCALURL)

help: ## Print this help
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
