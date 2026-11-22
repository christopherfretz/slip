#include "side.h"
// §5.7 ColdDeclData `binding_var`: the holding variable of a tuple-like binding, against an aggregate binding.
namespace std { template <class> struct tuple_size; template <unsigned long, class> struct tuple_element; }
struct P { int x; template <unsigned long I> int get() const { return x; } };
#ifdef B
template <> struct std::tuple_size<P> { static constexpr unsigned long value = 1; };
template <> struct std::tuple_element<0, P> { using type = int; };
#endif
P p;
auto [a] = p;
