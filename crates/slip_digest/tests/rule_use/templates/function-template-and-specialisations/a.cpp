template <class T> int f(T) { return 1; }
template <> int f(char) { return 2; }
int use() { return f(1) + f(2L) + f('c'); }
