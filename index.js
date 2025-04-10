const {
  Mnemonic,
  SecretKey,
  Signature,
  SpendBundle,
  toHex,
  encodeOffer,
} = require("chia-wallet-sdk");

const dotenv = require("dotenv");
const axios = require("axios");

dotenv.config();

const sk = SecretKey.fromSeed(new Mnemonic(process.env.MNEMONIC).toSeed(""));

async function main() {
  const fakeOffer = new SpendBundle([], Signature.infinity());

  const {
    data: { code },
  } = await axios.post(`${process.env.BASE_URL}/upload_offer`, {
    offer: encodeOffer(fakeOffer),
    signature: toHex(sk.sign(fakeOffer.hash()).toBytes()),
  });

  console.log("Offer code is", code);

  const {
    data: { offer },
  } = await axios.post(`${process.env.BASE_URL}/download_offer`, {
    code,
  });

  console.log("Offer is", offer);
}

main();
