#include "dep.h"

namespace legacy {
struct Mine : legacy::util::Tag { int a, b; };

template <unsigned long I> int &get(Mine &m) { return I == 0 ? m.a : m.b; }
}
template <> struct std::tuple_size<legacy::Mine> { static const unsigned long value = 2; };
template <unsigned long I> struct std::tuple_element<I, legacy::Mine> { using type = int; };

int user(legacy::Mine &m) {
  auto &[x, y] = m;
  return x + y;
}
