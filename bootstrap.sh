wget http://nlp.stanford.edu/data/glove.6B.zip
unzip -d "glove.6B" glove.6B.zip
rm -rf glove.6B/glove.6B.100d.txt glove.6B/glove.6B.200d.txt glove.6B/glove.6B.300d.txt glove.6B.zip
echo "Finished!"
