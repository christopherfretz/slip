namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> struct W {
  T v;
  W(T x) : v(x) {}
  ~W() {}
  W &operator=(const W &o) { v = o.v; return *this; }
  bool operator==(const W &o) const { return v == o.v; }
  friend int operator""_w(unsigned long long n) { return int(n); }
};
int operator""_w(unsigned long long);
int f(W<int> a, W<int> b) { a = b; return (a == b) + 3_w; }
