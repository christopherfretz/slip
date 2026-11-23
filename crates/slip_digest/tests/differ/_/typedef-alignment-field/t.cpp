#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): another aligned typedef of another alignment.
typedef int I __attribute__((aligned(8)));
typedef int J __attribute__((aligned(16)));
#ifdef B
struct S { char c; J i; };
#else
struct S { char c; I i; };
#endif
