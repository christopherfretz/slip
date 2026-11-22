#include "side.h"
// §11.9 (WP-026): a function named by an explicit instantiation declaration is defined only when used.
template <class T> inline int f(T) { return 1; }
extern template int f<int>(int);
#ifdef B
int y() { return f(1); }
#else

#endif
