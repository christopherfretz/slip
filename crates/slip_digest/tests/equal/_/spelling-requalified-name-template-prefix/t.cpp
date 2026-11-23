#include "side.h"
// §4.4 "Spellings", rule 10: a prefix with template arguments that hold nothing beyond the referent: `Bq<1>::value` against `::m::Bq<1>::value`, in a body and in a signature.
namespace m { template<int N> struct Bq { static constexpr int value = 1; }; }
using m::Bq;
template<int N> struct A {};
#ifdef B
template<class T> int g(T) { return ::m::Bq<1>::value; }
template<class T> auto k(T t) -> A<m::Bq<1>::value>;
#else
template<class T> int g(T) { return Bq<1>::value; }
template<class T> auto k(T t) -> A<Bq<1>::value>;
#endif
