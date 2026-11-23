template <class T> int closure_size() {
  const T n = 4;
  auto get = [&n] { return n; };
  return sizeof(get);
}
int user() { return closure_size<int>(); }
