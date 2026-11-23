namespace std {
template <class T> struct tuple_size;
template <decltype(sizeof 0) I, class T> struct tuple_element;
}
namespace app {
struct Pair {
  int a = 1, b = 2;
  template <decltype(sizeof 0) I> int get() const { return I ? b : a; }
};
template <decltype(sizeof 0) I> int get(const Pair &p) { return I ? p.a : p.b; }
}
template <> struct std::tuple_size<app::Pair> { static constexpr decltype(sizeof 0) value = 2; };
template <decltype(sizeof 0) I> struct std::tuple_element<I, app::Pair> { using type = int; };

int first(const app::Pair &p) {
  auto [x, y] = p;
  return x;
}
