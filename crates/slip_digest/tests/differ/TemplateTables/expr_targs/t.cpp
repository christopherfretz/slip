#include "side.h"
// §9.5 TemplateTables `expr_targs`: the explicit template arguments of a
// dependent name.
#ifdef B
template <class T> int f() { return T::template g<long>(); }
#else
template <class T> int f() { return T::template g<int>(); }
#endif
