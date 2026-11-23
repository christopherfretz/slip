#include "side.h"
// §4.4 "Spellings", rule 5: an array parameter's adjusted type holds the canonical element type; its type as written is read from the parameter.
#ifdef B
namespace A2 { int f(int); template<class T> struct H { void k(T t, decltype(f(T()))[2]); }; }
namespace A1 { template<class T> struct G { void k(T t, decltype(f(T()))[2]); }; }
#else
namespace A1 { template<class T> struct G { void k(T t, decltype(f(T()))[2]); }; }
namespace A2 { int f(int); template<class T> struct H { void k(T t, decltype(f(T()))[2]); }; }
#endif
