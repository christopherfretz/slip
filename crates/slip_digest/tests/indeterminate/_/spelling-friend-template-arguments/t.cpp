// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, declaration forms: a friend function template specialisation by an unqualified template-id, `friend void h<..>();` (`spelling/friend-name`).
namespace ns { }
template<int N> struct Bq { Bq(); Bq(int); static void g(); struct Y {}; enum E { e1 }; enum class F : int; template<int> static void t(); template<int> struct Z { }; };
int f(int);
#ifdef B
long f(long);
#endif
template<int> void h();
template<class T> struct S { friend void h<sizeof(sizeof(f(T())))>(); };
