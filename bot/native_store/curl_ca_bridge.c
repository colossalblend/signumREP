#define _GNU_SOURCE
#include <curl/curl.h>
#include <dlfcn.h>
#include <stdlib.h>

CURL *curl_easy_init(void) {
    CURL *(*initialize)(void) = dlsym(RTLD_NEXT, "curl_easy_init");
    if (!initialize) return NULL;
    CURL *handle = initialize();
    const char *certificate = getenv("SSL_CERT_FILE");
    if (handle && certificate && *certificate) {
        if (curl_easy_setopt(handle, CURLOPT_CAINFO, certificate) != CURLE_OK) {
            curl_easy_cleanup(handle);
            return NULL;
        }
    }
    return handle;
}
