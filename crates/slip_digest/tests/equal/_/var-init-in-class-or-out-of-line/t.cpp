#include "side.h"
// §2.1, §2.3, WP-026: the entity as its declarations together define it:
//  the initialiser 1, whether written in the class or on the out-of-line
//  definition. A use that needs the in-class form (a constant expression
//  before the definition) is written where it is.
#ifdef B
struct S { static const int x; };
const int S::x = 1;
#else
struct S { static const int x = 1; };
const int S::x;
#endif
