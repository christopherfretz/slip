#include "side.h"
// spec/digest.md §1 rule 2: paren in declarator does not matter.
#ifdef B
int (*v);
#else
int* v;
#endif
