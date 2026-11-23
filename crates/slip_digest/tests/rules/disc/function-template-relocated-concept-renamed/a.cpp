namespace acme { template <class T> concept C = sizeof(T) > 1; }
template <acme::C T> int f(T) { return 1; }
template <class T> int f(T) { return 2; }
int use() { return f(1) + f('c'); }
