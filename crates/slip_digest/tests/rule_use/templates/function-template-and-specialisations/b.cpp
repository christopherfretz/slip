template <class T> int g(T) { return 1; }
template <> int g(char) { return 2; }
int use() { return g(1) + g(2L) + g('c'); }
