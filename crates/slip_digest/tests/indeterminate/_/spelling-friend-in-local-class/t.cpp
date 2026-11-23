// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, declaration forms: a qualified friend in a local class of a function template (`spelling/friend-name`).
namespace ns { }
template<int N> struct Bq { Bq(); Bq(int); static void g(); struct Y {}; enum E { e1 }; enum class F : int; template<int> static void t(); template<int> struct Z { }; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g() { struct L { friend void Bq<sizeof(sizeof(f(T())))>::g(); }; }
