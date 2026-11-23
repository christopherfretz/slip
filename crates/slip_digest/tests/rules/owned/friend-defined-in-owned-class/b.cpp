namespace acme { struct Mine { friend int hook(Mine) { return 1; } }; }
int use() { return hook(acme::Mine()); }
