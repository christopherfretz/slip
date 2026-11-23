int g(int) { return 1; }
int g(long) { return 2; }
int h(char) { return 3; }
int use() { return g(1) + g(2L) + h('c'); }
