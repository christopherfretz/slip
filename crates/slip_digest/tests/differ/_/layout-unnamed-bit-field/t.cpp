#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an unnamed bit-field moves the next field; it is a member of the class.
#ifdef B
struct S { char a; int : 0; char b; };
#else
struct S { char a; char b; };
#endif
