#include "side.h"
// digest §6 _ `expr-template-args-as-written`.
template<class T> int tf(T);
#ifdef B
int v = tf(1);
#else
int v = tf<int>(1);
#endif
