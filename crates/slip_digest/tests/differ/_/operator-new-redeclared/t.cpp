#include "side.h"
// WP-050: Kiln declares the global allocation functions without a
// location before the first declaration of one; a redeclaration in source
// makes one declared unit, in this file (digest §11 origin file), and
// changes nothing else: the implicit ones nothing refers to stay no unit,
// nor does the implicit `std` they are declared with (§2.2).
#ifdef B
void *operator new(decltype(sizeof 0));
#endif
int x;
