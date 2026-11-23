// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, declaration forms: a friend function named through a qualifier, `friend void Bq<..>::g();`: Kiln resolves it to `Bq<8>::g` and keeps no written qualifier (`spelling/friend-name`).
namespace ns { }
template<int N> struct Bq { Bq(); Bq(int); static void g(); struct Y {}; enum E { e1 }; enum class F : int; template<int> static void t(); template<int> struct Z { }; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { friend void Bq<sizeof(sizeof(f(T())))>::g(); };
