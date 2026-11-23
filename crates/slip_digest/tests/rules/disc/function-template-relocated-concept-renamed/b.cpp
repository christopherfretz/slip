namespace legacy { template <class T> concept C = sizeof(T) > 1; }
template <legacy::C T> int g(T) { return 1; }
template <class T> int f(T) { return 2; }
int use() { return g(1) + f('c'); }
