namespace legacy {
struct Dep {
  template <class T> static int get(const T &t) { return t.s; }
};
}
