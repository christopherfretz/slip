#include "side.h"
// §9.5 TemplateTables `ref_var_insts` (§11.8): a use of a variable template
// specialisation records an instantiation; it changes no unit both sides have.
template <class T> int v = sizeof(T);
#ifdef B
int h() { return v<int>; }
#else

#endif
