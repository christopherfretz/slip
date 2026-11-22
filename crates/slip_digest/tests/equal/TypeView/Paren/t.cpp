#include "side.h"
// §4.3 TypeView `Paren`: sugar.
#ifdef B
int (*v);
#else
int* v;
#endif
