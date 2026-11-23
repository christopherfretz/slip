// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, declaration forms: a class-scope explicit specialisation of a member function template in a class template: Kiln keeps the converted argument (8), not the expression written (`spelling/specialisation-arguments`).
namespace ns { }
template<int N> struct Bq { Bq(); Bq(int); static void g(); struct Y {}; enum E { e1 }; enum class F : int; template<int> static void t(); template<int> struct Z { }; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { template<int> void h(); template<> void h<sizeof(sizeof(f(T())))>() { } };
