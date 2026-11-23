namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> int g() {
  if constexpr (sizeof(T) > 100) { struct A { int a = 1; }; return A{}.a; }
  struct L { int v = 2; int get() const { return v; } };
  return L{}.get();
}
int f() { return g<int>(); }
