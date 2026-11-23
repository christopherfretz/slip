#include "side.h"
// digest §3.4, WP-039 item 1: parameters of function pointer type differing in noexcept: two keys.
void f(void (*)() noexcept);
#ifdef B
void f(void (*)());
#endif
