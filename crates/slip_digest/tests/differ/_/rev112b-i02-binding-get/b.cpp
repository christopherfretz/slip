// rev-112b probe: a structured binding's implicit get<I>() call uses a default that consumes the caller's name
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct B { int v; template<int I> int get(unsigned n = len(__builtin_FUNCTION())) const; };
namespace std { template<class> struct tuple_size; template<unsigned long, class> struct tuple_element; }
template<> struct std::tuple_size<B> { static constexpr unsigned long value = 1; };
template<> struct std::tuple_element<0, B> { using type = int; };
void start() { auto [x] = B{1}; take(x); }
