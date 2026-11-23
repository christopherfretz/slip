namespace legacy {

template <class T> struct store {
  void put(T &t) { prepare(t); }
};
}
