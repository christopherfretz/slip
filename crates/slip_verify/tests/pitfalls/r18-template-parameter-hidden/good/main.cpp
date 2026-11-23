struct Base {
  using U = long;
};
template <class V> struct S : Base {
  V get() { return 0; }
};

int width() { return sizeof(S<int>().get()); }
