// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, declaration forms: a class-scope explicit specialisation of a member variable template (`spelling/specialisation-arguments`).
namespace ns { }
template<int N> struct Bq { Bq(); Bq(int); static void g(); struct Y {}; enum E { e1 }; enum class F : int; template<int> static void t(); template<int> struct Z { }; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { template<int> static const int v = 0; template<> const int v<sizeof(sizeof(f(T())))> = 1; };
