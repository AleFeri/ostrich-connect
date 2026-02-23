#pragma once

#ifdef __cplusplus
extern "C" {
#endif

typedef struct BackendHandle BackendHandle;

BackendHandle *oc_backend_new(void);
char *oc_backend_execute(BackendHandle *handle, const char *command_json);
void oc_string_free(char *ptr);
void oc_backend_free(BackendHandle *handle);

#ifdef __cplusplus
}
#endif
