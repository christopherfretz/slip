namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> int g() {
  if constexpr (sizeof(T) > 100) { return [] { return 1; }(); }
  return [] { return 2; }();
}
int f() { return g<int>(); }
