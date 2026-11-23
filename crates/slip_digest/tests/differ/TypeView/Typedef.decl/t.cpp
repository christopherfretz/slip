#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): another aligned typedef of the same alignment (digest-kinds §4.2 rule 9: the typedef's entity is written, so a rename maps it by a rule; without one the field differs, a false difference).
typedef int I __attribute__((aligned(8)));
typedef int K __attribute__((aligned(8)));
#ifdef B
struct S { char c; K i; };
#else
struct S { char c; I i; };
#endif
static_assert(alignof(S) == 8 && sizeof(S) == 16);
