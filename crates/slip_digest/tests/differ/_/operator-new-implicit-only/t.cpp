#include "side.h"
// WP-050: a new-expression with no declaration of `operator new` in
// source refers to Kiln's implicit one, which has no location: a derived
// unit (§2.2), no declared one.
#ifdef B
int *p = new int;
#endif
