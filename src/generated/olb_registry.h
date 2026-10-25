/* Auto-generated C header for olb_registry FFI. DO NOT EDIT. */
#ifndef OLB_REGISTRY_FFI_H
#define OLB_REGISTRY_FFI_H

#ifdef __cplusplus
extern "C" {
#endif

/* anchor instruction */
char* olb_registry_anchor(const char* args_json);

/* fetch registry account state */
char* olb_registry_fetch_registry(const char* args_json);

/* wallet helpers */
char* olb_registry_check_connection(const char* args_json);
char* olb_registry_inspect_account(const char* args_json);
char* olb_registry_list_accounts(const char* args_json);
char* olb_registry_create_account(const char* args_json);

void olb_registry_free_string(char* s);
char* olb_registry_version(void);
char* olb_registry_program_id(void);
char* olb_registry_decode_account(const char* args_json);

#ifdef __cplusplus
}
#endif

#endif /* OLB_REGISTRY_FFI_H */
