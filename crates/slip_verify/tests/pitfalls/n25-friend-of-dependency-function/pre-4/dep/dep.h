namespace legacy {
template <class T> auto peek(const T &t) -> decltype(t.secret) { return t.secret; }
inline int peek(...) { return 0; }
}
