#include "side.h"
// §5.4 VarData `templ`, §9.1.
#ifdef B
template <class T> int v = 0;
template <> int v<int> = 1;
#else
template <class T> int v = 0;
template <> int v<int>;
#endif
