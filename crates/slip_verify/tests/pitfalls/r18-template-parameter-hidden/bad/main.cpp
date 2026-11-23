struct Base {
  using U = long;
};
template <class U> struct S : Base {
  U get() { return 0; }
};

int width() { return sizeof(S<int>().get()); }
