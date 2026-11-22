#include "side.h"
// digest §2.1: an explicit specialisation of a variable template is written in source and is listed.
template <class T> int v = sizeof(T);
#ifdef B
int a = 1;
template <> int v<int> = 2;
#else
template <> int v<int> = 2;
int a = 1;
#endif
