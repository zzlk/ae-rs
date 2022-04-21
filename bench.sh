sudo nice -n-10 sudo -u $(logname) cargo bench -- --measurement-time 30 -n
echo "Benchmark Commit" >/tmp/txt
echo "" >>/tmp/txt
critcmp base.json new >> /tmp/txt
critcmp --export base > base.json
git add base.json
git commit --allow-empty -F /tmp/txt
