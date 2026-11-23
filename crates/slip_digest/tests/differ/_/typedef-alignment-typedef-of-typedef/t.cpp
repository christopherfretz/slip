#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a plain typedef of the typedef, and its use.
typedef int I __attribute__((aligned(8)));
#ifdef B
typedef I T2;
#else
typedef int T2;
#endif
struct S { char c; T2 i; };
