// WP-115 item 1 (digest §9): the file name "get" and get()'s plain name are one string object; it has a file-name use, so its bytes are written: G (the file name) is equal under the rename of get(), which changes no file name
consteval const char *get() { (void)__builtin_FUNCTION();
#include "get"
}
constexpr const char *G = get();
