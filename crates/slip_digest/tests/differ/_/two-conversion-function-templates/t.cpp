#include "side.h"
// digest-kinds rule §3.2: three conversion function templates of one class.
struct S {
#ifdef B
template <class T> operator T*(); template <class T> operator T(); template <class T> operator const T();
#else
template <class T> operator T*(); template <class T> operator T();
#endif
};
