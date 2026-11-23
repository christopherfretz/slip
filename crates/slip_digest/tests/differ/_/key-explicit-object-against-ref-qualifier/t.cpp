#include "side.h"
// std: c++23
// digest §3.4, WP-039 item 1: an explicit object parameter against a ref-qualified member: two keys.
struct S { void f() &;
#ifdef B
void f(this S&, int);
#endif
};
