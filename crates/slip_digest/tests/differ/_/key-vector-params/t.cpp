#include "side.h"
// digest §3.4, WP-039 item 1: vector parameters of two sizes: two keys.
void f(int __attribute__((vector_size(16))));
#ifdef B
void f(int __attribute__((vector_size(32))));
#endif
