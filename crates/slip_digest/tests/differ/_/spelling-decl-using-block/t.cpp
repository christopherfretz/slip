// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, declaration forms (the audit of fix round 5): a block-scope using-declaration `using Bq<..>::e1;`. Side B adds `f(long)`.
namespace ns { }
template<int N> struct Bq { Bq(); Bq(int); static void g(); struct Y {}; enum E { e1 }; enum class F : int; template<int> static void t(); template<int> struct Z { }; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g() { using Bq<sizeof(sizeof(f(T())))>::e1; }
