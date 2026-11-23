#include "dep.h"

namespace acme {
struct Mine : legacy::util::Tag { int a, b; };
} namespace legacy {
template <unsigned long I> int &get(acme::Mine &m) { return I == 0 ? m.a : m.b; }
}
template <> struct std::tuple_size<acme::Mine> { static const unsigned long value = 2; };
template <unsigned long I> struct std::tuple_element<I, acme::Mine> { using type = int; };

int user(acme::Mine &m) {
  auto &[x, y] = m;
  return x + y;
}
