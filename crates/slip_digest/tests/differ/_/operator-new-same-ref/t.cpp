#include "side.h"
// WP-050 (A3): the implicit `operator new` and the one a source
// redeclares have one key, so a function that calls it is the same unit
// whether or not the translation unit redeclares it (a header shared by
// a unit that includes `<new>` and one that does not).
#ifdef B
void *operator new(decltype(sizeof 0));
#endif
inline void *f() { return ::operator new(1); }
void *g() { return f(); }
