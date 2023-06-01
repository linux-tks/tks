
Own your TPM
============

TKS uses the TPM to manipulate and encrypt your secrets. This is only possible
if you own your TPM. You can check this by running the following command:

```shell
$ tss2_list
```

In case you get an error message, you need to take ownership of your TPM. This
can be done following these steps:

. Boot the system into the BIOS
. Find the option to clear the TPM
. Clear the TPM
. After the system has booted, run the following command:

```shell
$ tss2_provision
```

