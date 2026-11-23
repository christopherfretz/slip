struct Base {
  using U = long;
};
template <class T> struct S : Base {
  T get() { return 0; }
};

int width() { return sizeof(S<int>().get()); }
