#include "side.h"
// §5.7 DeclArena `fn_def_insts` (demand, §11): a use that instantiates a function definition leaves every unit equal.
template <class T> T f(T t) { return t; }
#ifdef B
int y() { return f(1); }
#endif
