#include "side.h"
// §4.4 "Spellings", rule 10: requalifying a resolved name in a dependent signature (`S::v` against `::n::S::v`, `value` against `Bq<1>::value` from inside `Bq<1>`) changes no digest.
namespace n { struct S { static constexpr int v = 1; }; }
using n::S;
template<int N> struct Bq;
template<> struct Bq<1> {
  static constexpr int value = 1;
#ifdef B
  template<class T> auto k(T t) -> decltype(t + Bq<1>::value);
#else
  template<class T> auto k(T t) -> decltype(t + value);
#endif
};
#ifdef B
template<class T> auto g(T t) -> decltype(t + ::n::S::v);
#else
template<class T> auto g(T t) -> decltype(t + S::v);
#endif
