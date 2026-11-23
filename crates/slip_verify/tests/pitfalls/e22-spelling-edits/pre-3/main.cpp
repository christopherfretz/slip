struct Elem {
  int n;
  Elem(long) : n(1) {}
  explicit Elem(int) : n(2) {}
};
template <class T> struct Vec {
  T last{0L};
  void push_back(const T &t) { last = t; }
  template <class... A> void emplace_back(A... a) { last = T(a...); }
};

int fill() { Vec<Elem> v; v.push_back(5); return v.last.n; }
