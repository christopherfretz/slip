template <class T> struct Crate {
  T v;
  Crate(T x) : v(x) {}
};
Crate(int) -> Crate<long>;
template <class T> using Alias = Crate<T>;

int caller() { Alias a{1}; return sizeof(a.v); }
