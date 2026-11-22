#include "side.h"
// §9.5 TemplateTables `end_of_tu_defs` (§11.8): a use that makes Kiln
// instantiate a definition at the end of the translation unit changes no unit
// both sides have.
template <class T> T f(T t) { return t; }
#ifdef B
int h() { return f(1); }
#else

#endif
