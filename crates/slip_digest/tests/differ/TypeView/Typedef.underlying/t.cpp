#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the same aligned typedef around another canonical type (digest-kinds §4.2 rule 9).
typedef long L __attribute__((aligned(4)));
#ifdef B
struct S { char c; L a[2]; };
#else
struct S { char c; L a; };
#endif
