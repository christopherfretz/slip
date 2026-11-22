// std: c++20
#include "side.h"
// §5.4 FnSpec `0-return-deduced`, §11.9: a comparison defaulted on its first declaration deduces its return type at the class's end, so the bit is in its own stream.
namespace std {
struct strong_ordering {
  int v;
  constexpr strong_ordering(int x) : v(x) {}
  constexpr operator int() const { return v; }
  static const strong_ordering less, equal, greater;
};
constexpr strong_ordering strong_ordering::less = -1;
constexpr strong_ordering strong_ordering::equal = 0;
constexpr strong_ordering strong_ordering::greater = 1;
}
#ifdef B
struct S { int i; auto operator<=>(const S&) const = default; };
#else
struct S { int i; std::strong_ordering operator<=>(const S&) const = default; };
#endif
