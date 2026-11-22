#include "side.h"
// §4.3 TypeView `Decltype`: sugar (§4.2 rule 7).
int x;
#ifdef B
decltype(x) v;
#else
int v;
#endif
