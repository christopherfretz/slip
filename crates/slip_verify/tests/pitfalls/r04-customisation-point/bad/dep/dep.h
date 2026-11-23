namespace lib {
template <class T> void prepare(T &) {}
template <class T> struct store {
  void put(T &t) { prepare(t); }
};
}
