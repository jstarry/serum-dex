include ../Makefile

LIB_NAME=serum_registry_program

.PHONY: test test-program deploy-lockup deploy-stake

test: deploy-lockup deploy-stake deploy-super test-program
	@ # no-op

# deploy-retbuf: build-retbu
# 	$(eval TEST_RETBUF_PROGRAM_ID=$(shell solana deploy $(SOL_OPTIONS) ../retbuf/target/bpfel-unknown-unknown/release/serum_retbuf.so --use-deprecated-loader | jq .programId -r))

# build-retbuf:
# 	cd ../ && ./do.sh build retbuf && cd ./registry

deploy-stake:
	$(eval TMP=$(shell make -s -C ../stake deploy))
	$(eval TEST_STAKE_PROGRAM_ID=$(shell echo $(TMP) | sed 's/.*{programId: \(.*\)}.*/\1/g'))

deploy-lockup:
	$(eval TMP=$(shell make -s -C ../lockup deploy))
	$(eval TEST_LOCKUP_PROGRAM_ID=$(shell echo $(TMP) | sed 's/.*{programId: \(.*\)}.*/\1/g'))

test-program:
	make \
	TEST_PROGRAM_ID=$(TEST_PROGRAM_ID) \
	TEST_LOCKUP_PROGRAM_ID=$(TEST_LOCKUP_PROGRAM_ID) \
	TEST_STAKE_PROGRAM_ID=$(TEST_STAKE_PROGRAM_ID) \
	test-program-super
