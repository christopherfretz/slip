#include "side.h"
// WP-041 (§2.4, §4.4 "Spellings", rule 4): a block-scope typedef writes its aliased type, used or not (was equal under WP-034, when a typedef wrote nothing).
#ifdef B
void f() { typedef long I; }
#else
void f() { typedef int I; }
#endif
