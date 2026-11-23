#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an aligned typedef of another aligned typedef, `I` or `K` (same alignment): every typedef met is listed (rule 9), not only the outermost, which decides the alignment.
typedef int I __attribute__((aligned(8)));
typedef int K __attribute__((aligned(8)));
#ifdef B
typedef K J __attribute__((aligned(16)));
#else
typedef I J __attribute__((aligned(16)));
#endif
J x;
