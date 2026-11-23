namespace legacy {
template <class T> using P = T *;
}
legacy::P<int> use(legacy::P<int> p) { return p; }
