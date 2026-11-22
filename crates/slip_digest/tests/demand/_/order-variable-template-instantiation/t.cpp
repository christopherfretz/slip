#include "side.h"
// digest §2.1: a use that instantiates a variable template does not list the specialisation; the order unit stays equal.
template <class T> int v = sizeof(T);
int a = 1;
#ifdef B
int y() { return v<int>; }
#endif
int b = 2;
